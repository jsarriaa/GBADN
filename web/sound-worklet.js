// Plays stereo sample blocks posted by the page, with silence when they run out.
class SoundPlayer extends AudioWorkletProcessor {
  constructor() {
    super();
    this.blocks = [];
    this.position = 0;
    this.port.onmessage = (event) => {
      // Dropping sound keeps it from lagging behind the picture.
      if (this.blocks.length < 12) {
        this.blocks.push(event.data);
      }
    };
  }

  process(inputs, outputs) {
    const [left, right] = outputs[0];
    for (let frame = 0; frame < left.length; frame += 1) {
      const block = this.blocks[0];
      if (!block) {
        left[frame] = 0;
        if (right) right[frame] = 0;
        continue;
      }
      left[frame] = block[this.position] / 32768;
      if (right) right[frame] = block[this.position + 1] / 32768;
      this.position += 2;
      if (this.position >= block.length) {
        this.blocks.shift();
        this.position = 0;
      }
    }
    return true;
  }
}

registerProcessor("sound-player", SoundPlayer);
