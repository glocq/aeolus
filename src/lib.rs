use nih_plug::prelude::*;
use std::sync::Arc;
use aubio::Pitch;

pub mod utils;
use crate::utils::*;


// Those are temporarily constants, but should eventually be turned into parameters:
const BUFFER_SIZE:  usize            = 128;
const HOP_SIZE:     usize            = 64;
const PITCH_METHOD: aubio::PitchMode = aubio::PitchMode::Yinfast;
const SAMPLE_RATE:  u32              = 44100;
const MIN_PITCH:    f32              = 57.0;
const MAX_PITCH:    f32              = 81.0;

// This is a shortened version of the gain example with most comments removed, check out
// https://github.com/robbert-vdh/nih-plug/blob/master/plugins/examples/gain/src/lib.rs to get
// started

struct Aeolus {
    params: Arc<AeolusParams>,
    pending_samples: Vec<f32>,
    pending_index: usize,
    pitch_analyzer: aubio::Result<Pitch>,
}

#[derive(Params)]
struct AeolusParams {
    /// The parameter's ID is used to identify the parameter in the wrappred plugin API. As long as
    /// these IDs remain constant, you can rename and reorder these fields as you wish. The
    /// parameters are exposed to the host in the same order they were defined. In this case, this
    /// gain parameter is stored as linear gain while the values are displayed in decibels.
    #[id = "gain"]
    pub gain: FloatParam,
}

impl Default for Aeolus {
    fn default() -> Self {
        Self {
            params: Arc::new(AeolusParams::default()),
            pending_samples: Vec::new(),
            pending_index: 0,
            pitch_analyzer: Err(aubio::Error::FailedInit),
        }
    }
}

impl Default for AeolusParams {
    fn default() -> Self {
        Self {
            // This gain is stored as linear gain. NIH-plug comes with useful conversion functions
            // to treat these kinds of parameters as if we were dealing with decibels. Storing this
            // as decibels is easier to work with, but requires a conversion for every sample.
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    // This makes the range appear as if it was linear when displaying the values as
                    // decibels
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            // Because the gain parameter is stored as linear gain instead of storing the value as
            // decibels, we need logarithmic smoothing
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            // There are many predefined formatters we can use here. If the gain was stored as
            // decibels instead of as a linear gain value, we could have also used the
            // `.with_step_size(0.1)` function to get internal rounding.
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}



// The `Plugin` trait requires our `Aeolus` type to implement `Send`. It does not automatically do so
// because it contains a `Pitch` field, which contains a raw pointer (see [1]).
// From what I understand, this page [2] suggests that it's okay to implement `Send`
// for a type that contains raw pointers, you just have to trust the (aubio-rs) library author...
// which I do, otherwise I wouldn't be using the library?
// I may have misunderstood something here, but this unsafe implementation is the only way forward
// that I see, so I'll go with it and wait to see if something goes wrong.
// [1] https://github.com/katyo/aubio-rs/blob/4697a1424f6e856ffbe91045a794529d4ecde8a8/src/pitch.rs#L210
// [2] https://doc.rust-lang.org/nomicon/send-and-sync.html
unsafe impl Send for Aeolus {}



impl Plugin for Aeolus {
    const NAME: &'static str = "Aeolus";
    const VENDOR: &'static str = "Grégoire Locqueville";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "gregoireloc@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // The first audio IO layout is used as the default. The other layouts may be selected either
    // explicitly or automatically by the host or the user depending on the plugin API/backend.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(1),
        main_output_channels: NonZeroU32::new(1),

        aux_input_ports: &[],
        aux_output_ports: &[],

        // Individual ports and the layout as a whole can be named here. By default these names
        // are generated as needed. This layout will be called 'Stereo', while a layout with
        // only one input and output channel would be called 'Mono'.
        names: PortNames::const_default(),
    }];


    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    // If the plugin can send or receive SysEx messages, it can define a type to wrap around those
    // messages here. The type implements the `SysExMessage` trait, which allows conversion to and
    // from plain byte buffers.
    type SysExMessage = ();
    // More advanced plugins can use this to run expensive background tasks. See the field's
    // documentation for more information. `()` means that the plugin does not have any background
    // tasks.
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // Resize buffers and perform other potentially expensive initialization operations here.
        // The `reset()` function is always called right after this function. You can remove this
        // function if you do not need it.
        self.pending_samples.resize(128, 0.0);
        self.pitch_analyzer = Pitch::new(
            PITCH_METHOD,
            BUFFER_SIZE,
            HOP_SIZE,
            SAMPLE_RATE,
        );
        true
    }

    fn reset(&mut self) {
        self.pending_index = 0;
        // It does not seem to be possible to reset the state of an `aubio::Pitch`,
        // so we won't do anything with it. It shouldn't make a difference
        // once the supposedly small time that it takes to play in a buffer's worth
        // of audio has elapsed.
        // We could manually feed as many zeroes as needed to the object,
        // but I don't think it's worth the hassle, so we don't do anything about that.
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut sample_index = 0; // will be incremented at each new sample in the buffer
        for channel_samples in buffer.iter_samples() {
            // Add a sample into the buffer of pending audio
            self.pending_samples[self.pending_index] = *channel_samples.into_iter().next().unwrap();
            self.pending_index += 1;
            // If the buffer of pending is filled, perform pitch analysis (if possible)
            if self.pending_index >= HOP_SIZE {
                match &mut self.pitch_analyzer {
                    Err(_)                   => {} // pitch analyzer not available
                    Ok(analyzer) => {
                        match analyzer.do_result(&self.pending_samples) {
                            Err(_) => {} // no pitch found
                            Ok(frequency) => {
                                context.send_event(NoteEvent::MidiCC {
                                    timing: sample_index,
                                    channel: 0,
                                    cc: 1,
                                    value: limit(
                                        scale(
                                            freq_to_midi(frequency),
                                                MIN_PITCH, MAX_PITCH, 0.0, 127.0
                                        ), 0.0, 127.0
                                    ),
                                })
                            }
                        };
                    }
                }
                self.pending_index = 0;
            }
            sample_index += 1;
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Aeolus {
    const CLAP_ID: &'static str = "io.github.glocq";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("to MIDI");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    // TODO Don't forget to change these features
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::NoteDetector,
        ClapFeature::Analyzer,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Aeolus {
    const VST3_CLASS_ID: [u8; 16] = *b"AeolusAAAAAAAAAA";

    // TODO And also don't forget to change these categories
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(Aeolus);
nih_export_vst3!(Aeolus);
