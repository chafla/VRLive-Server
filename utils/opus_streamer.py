import ctypes
import sys
from os import path

from rtp import RTP, Extension, PayloadType

import wave


from pyogg import opus

import opuslib.classes


class WaveToOpus:

    def __init__(self, filename: str, loop_forever=True):
        self.loop_forever = loop_forever
        self._wave_file = None
        self._packet_dur_ms = 20
        fn, ext = path.splitext(filename)

        if ext == ".mp3":
            from pydub import AudioSegment
            sound = AudioSegment.from_mp3(filename)

            filename = f"{fn}.wav"
            sound = sound.set_frame_rate(48000)
            sound.export(filename, format="wav")
        # y, samples_per_second = librosa.load(filename, sr=48000)  # Upsample 44.1kHz

        self._wave_file = wave.open(filename, "rb")

        self.channels = self._wave_file.getnchannels()
        print("Number of channels:", self.channels)
        self.samples_per_second = self._wave_file.getframerate()
        print("Sampling frequency:", self.samples_per_second)
        # librosa.
        # librosa.samples_to_frames(y)
        self.bytes_per_sample = self._wave_file.getsampwidth()

        if self.samples_per_second != 48000:
            raise NotImplementedError("GUH WE ONLY SUPPORT THIS FOR NOW")

        self.encoder = opuslib.Encoder(self.samples_per_second, self.channels, opus.OPUS_APPLICATION_VOIP)

        # Calculate the desired frame size (in samples per channel)
        self.desired_frame_duration = self._packet_dur_ms / 1000  # milliseconds
        self.desired_frame_size = int(self.desired_frame_duration * self.samples_per_second)
        self.running_timestamp = 0

    def __del__(self):
        if self._wave_file is not None:
            self._wave_file.close()

    def get_next_packet(self):
        # Get data from the wav file
        pcm = self._wave_file.readframes(self.desired_frame_size)

        # Check if we've finished reading the wav file
        if len(pcm) == 0:
            return None

        # Calculate the effective frame size from the number of bytes
        # read
        effective_frame_size = (
                len(pcm)  # bytes
                // self.bytes_per_sample
                // self.channels
        )

        # see: https://www.opus-codec.org/docs/html_api/group__opusencoder.html
        # The passed frame_size must an opus frame size for the encoder's sampling rate.
        # For example, at 48kHz the permitted values are 120, 240, 480, or 960.
        # Passing in a duration of less than 10ms (480 samples at 48kHz) will prevent the encoder from using the LPC
        # or hybrid modes.

        if self.samples_per_second != 48000:
            print("guh")

        # if samples_per_second not in []

        # Check if we've received enough data
        if effective_frame_size < self.desired_frame_size:
            # We haven't read a full frame from the wav file, so this
            # is most likely a final partial frame before the end of
            # the file.  We'll pad the end of this frame with silence.
            pcm += (
                    b"\x00"
                    * ((self.desired_frame_size - effective_frame_size)
                       * self.bytes_per_sample
                       * self.channels)
            )

            if self.loop_forever:
                self._wave_file.setpos(0)

        # csrc_list =

        # Encode the PCM data
        encoded_packet = self.encoder.encode(pcm, self.desired_frame_size)
        # encoded_packet = opus. opus_encoder.encode(pcm)

        # sample rate += sampling rate / packets per second
        self.running_timestamp += self.samples_per_second / (1 / (self._packet_dur_ms / 1000))
        base_rtp = RTP(
            marker=True,
            # https://lmtools.com/content/rtp-timestamp-calculation
            timestamp=int(self.running_timestamp)
        )

        base_rtp.payload = bytearray(encoded_packet)
        # bytes_encoded += len(encoded_packet)

        return base_rtp

    # At this stage we now have a buffer containing an
    # Opus-encoded packet.  This could be sent over UDP, for
    # example, and then decoded with OpusDecoder.  However it
    # cannot really be saved to a file without wrapping it in the
    # likes of an Ogg stream; for this see OggOpusWriter.


if __name__ == '__main__':
    handler = WaveToOpus("howd_i_wind_up_here.mp3" if len(sys.argv) == 1 else sys.argv[1])
    packets = 0
    while True:
        packets += 1
        pkt = handler.get_next_packet()
        if pkt is None:
            break

    print(f"{packets} packets handled")
