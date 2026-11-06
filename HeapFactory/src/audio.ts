/**
 * All sound is synthesised with WebAudio: no asset files.
 * Wind through sheet metal, distant gulls, creaking girders, a slow melancholy pad,
 * a faraway radio, and small sounds for everything Lazlo does.
 */
export class Sound {
  private ctx: AudioContext | null = null
  private master!: GainNode
  private sfxBus!: GainNode
  private windGain!: GainNode
  private windFilter!: BiquadFilterNode
  private musicGain!: GainNode
  private radioGain!: GainNode
  private lowGain!: GainNode
  private noise!: AudioBuffer
  private nextGull = 6
  private nextCreak = 4
  private nextChord = 0
  private chordIndex = 0
  private t = 0
  private track: GainNode | null = null
  private radioTrack = false
  muted = false

  /** Must be called from a user gesture. */
  start() {
    if (this.ctx) {
      void this.ctx.resume()
      return
    }
    const ctx = new AudioContext()
    this.ctx = ctx
    this.master = ctx.createGain()
    this.master.gain.value = 0.8
    this.master.connect(ctx.destination)

    // a little shared reverb so everything sits in the same big open space
    const verb = ctx.createConvolver()
    verb.buffer = this.impulse(3.2)
    const verbGain = ctx.createGain()
    verbGain.gain.value = 0.35
    verb.connect(verbGain).connect(this.master)
    const send = (n: AudioNode, amt: number) => {
      const g = ctx.createGain()
      g.gain.value = amt
      n.connect(g).connect(verb)
    }

    this.sfxBus = ctx.createGain()
    this.sfxBus.gain.value = 0.7
    this.sfxBus.connect(this.master)
    send(this.sfxBus, 0.4)

    const len = ctx.sampleRate * 4
    this.noise = ctx.createBuffer(1, len, ctx.sampleRate)
    const d = this.noise.getChannelData(0)
    let b = 0
    for (let i = 0; i < len; i++) {
      // brownish noise: softer than white, more like moving air
      b = (b + 0.02 * (Math.random() * 2 - 1)) / 1.02
      d[i] = b * 3.5
    }

    // wind: looping noise through a wandering band-pass
    const wind = ctx.createBufferSource()
    wind.buffer = this.noise
    wind.loop = true
    this.windFilter = ctx.createBiquadFilter()
    this.windFilter.type = 'bandpass'
    this.windFilter.frequency.value = 500
    this.windFilter.Q.value = 0.7
    this.windGain = ctx.createGain()
    this.windGain.gain.value = 0.25
    wind.connect(this.windFilter).connect(this.windGain).connect(this.master)
    send(this.windGain, 0.2)
    wind.start()

    // music: sparse, slow pad chords
    this.musicGain = ctx.createGain()
    this.musicGain.gain.value = 0.11
    this.musicGain.connect(this.master)
    send(this.musicGain, 0.8)
    void this.loadTrack('/music/soundtrack.mp3', this.master).then((g) => (this.track = g))

    // the radio: a thin, wobbly music-box tune that fades in with distance
    this.radioGain = ctx.createGain()
    this.radioGain.gain.value = 0
    const radioFilter = ctx.createBiquadFilter()
    radioFilter.type = 'bandpass'
    radioFilter.frequency.value = 1400
    radioFilter.Q.value = 1.2
    this.radioGain.connect(radioFilter).connect(this.master)
    send(radioFilter, 0.3)
    // the Suno waltz plays through the radio's band-pass; the synth tune is the fallback
    void this.loadTrack('/music/radio.mp3', this.radioGain).then((g) => {
      if (!g) return this.radioLoop()
      g.gain.value = 0.45
      this.radioTrack = true
    })

    // low charge: a soft, slow two-note warning
    this.lowGain = ctx.createGain()
    this.lowGain.gain.value = 0
    this.lowGain.connect(this.master)
  }

  /** Loops a recorded track (from Suno) into `dest`. Returns its gain, or null if the file is missing. */
  private async loadTrack(url: string, dest: AudioNode) {
    const ctx = this.ctx!
    try {
      const res = await fetch(url)
      if (!res.ok || !res.headers.get('content-type')?.startsWith('audio')) return null
      const buf = await ctx.decodeAudioData(await res.arrayBuffer())
      const src = ctx.createBufferSource()
      src.buffer = buf
      src.loop = true
      const gain = ctx.createGain()
      gain.gain.value = 0
      src.connect(gain).connect(dest)
      src.start()
      return gain
    } catch {
      return null // keep the procedural fallback
    }
  }

  private impulse(seconds: number) {
    const ctx = this.ctx!
    const len = ctx.sampleRate * seconds
    const buf = ctx.createBuffer(2, len, ctx.sampleRate)
    for (let c = 0; c < 2; c++) {
      const d = buf.getChannelData(c)
      for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, 3)
    }
    return buf
  }

  private tone(freq: number, dur: number, opts: { type?: OscillatorType; gain?: number; attack?: number; dest?: AudioNode; when?: number; glide?: number; detune?: number } = {}) {
    const ctx = this.ctx
    if (!ctx) return
    const at = ctx.currentTime + (opts.when ?? 0)
    const o = ctx.createOscillator()
    o.type = opts.type ?? 'sine'
    o.frequency.setValueAtTime(freq, at)
    if (opts.glide) o.frequency.exponentialRampToValueAtTime(freq * opts.glide, at + dur)
    if (opts.detune) o.detune.value = opts.detune
    const g = ctx.createGain()
    const peak = opts.gain ?? 0.2
    const atk = opts.attack ?? 0.005
    g.gain.setValueAtTime(0.0001, at)
    g.gain.exponentialRampToValueAtTime(peak, at + atk)
    g.gain.exponentialRampToValueAtTime(0.0001, at + dur)
    o.connect(g).connect(opts.dest ?? this.sfxBus)
    o.start(at)
    o.stop(at + dur + 0.05)
  }

  private burst(dur: number, freq: number, q: number, gain: number, when = 0) {
    const ctx = this.ctx
    if (!ctx) return
    const at = ctx.currentTime + when
    const s = ctx.createBufferSource()
    s.buffer = this.noise
    const f = ctx.createBiquadFilter()
    f.type = 'bandpass'
    f.frequency.value = freq
    f.Q.value = q
    const g = ctx.createGain()
    g.gain.setValueAtTime(gain, at)
    g.gain.exponentialRampToValueAtTime(0.0001, at + dur)
    s.connect(f).connect(g).connect(this.sfxBus)
    s.start(at, Math.random() * 3, dur + 0.05)
  }

  private radioLoop() {
    const ctx = this.ctx
    if (!ctx || this.radioTrack) return
    // an old, half-remembered waltz: slightly out of tune
    const notes = [523, 659, 784, 659, 587, 698, 880, 698, 523, 659, 784, 1047, 988, 784, 659, 587]
    notes.forEach((n, i) => {
      this.tone(n, 0.9, { type: 'triangle', gain: 0.25, when: i * 0.42, dest: this.radioGain, detune: Math.sin(i) * 18 })
      if (i % 3 === 0) this.tone(n / 2, 1.2, { type: 'sine', gain: 0.15, when: i * 0.42, dest: this.radioGain })
    })
    setTimeout(() => this.radioLoop(), notes.length * 420 + 1600)
  }

  private gull() {
    // a descending, wavering cry, far away
    const n = 2 + Math.floor(Math.random() * 3)
    for (let i = 0; i < n; i++) {
      const f = 1500 + Math.random() * 400
      this.tone(f, 0.35, { type: 'sawtooth', gain: 0.012, attack: 0.04, glide: 0.62, when: i * 0.32, detune: (Math.random() - 0.5) * 60 })
    }
  }

  private creak() {
    // metal under strain: a slow, gritty, gliding resonance
    const f = 90 + Math.random() * 120
    this.tone(f, 1.6, { type: 'sawtooth', gain: 0.018, attack: 0.4, glide: 0.8 + Math.random() * 0.5 })
    this.tone(f * 2.71, 1.2, { type: 'sine', gain: 0.01, attack: 0.3, glide: 0.9 })
  }

  private chord() {
    // D minor 9 → Bb maj7 → F add9 → C sus: wistful, unresolved
    const chords = [
      [146.8, 220, 261.6, 349.2, 329.6],
      [116.5, 174.6, 220, 293.7, 349.2],
      [87.3, 174.6, 196, 261.6, 220],
      [130.8, 196, 261.6, 293.7, 392],
    ]
    const c = chords[this.chordIndex++ % chords.length]
    c.forEach((f, i) => {
      this.tone(f, 9, { type: 'sine', gain: 0.3 - i * 0.03, attack: 2.5, dest: this.musicGain, detune: (Math.random() - 0.5) * 8 })
      this.tone(f * 2, 7, { type: 'triangle', gain: 0.04, attack: 3, dest: this.musicGain, detune: 6 })
    })
    // an occasional single high note, like a distant bell
    if (Math.random() < 0.6) this.tone(c[3] * 2, 4, { type: 'sine', gain: 0.12, attack: 0.01, when: 2 + Math.random() * 3, dest: this.musicGain })
  }

  /** Call every frame. `height` above the heap floor raises the wind. */
  update(dt: number, s: { height: number; radio: number; low: number }) {
    const ctx = this.ctx
    if (!ctx) return
    this.t += dt
    const now = ctx.currentTime
    const gust = 0.5 + 0.5 * Math.sin(this.t * 0.23) * Math.sin(this.t * 0.071 + 1)
    const lift = Math.min(1, Math.max(0, s.height / 30))
    this.windGain.gain.setTargetAtTime(0.12 + gust * 0.22 + lift * 0.2, now, 0.5)
    this.windFilter.frequency.setTargetAtTime(280 + gust * 600 + lift * 300, now, 0.5)
    this.radioGain.gain.setTargetAtTime(s.radio * 0.5, now, 0.3)
    this.master.gain.setTargetAtTime(this.muted ? 0 : 0.8, now, 0.1)

    this.nextGull -= dt
    if (this.nextGull <= 0) {
      this.gull()
      this.nextGull = 9 + Math.random() * 16
    }
    this.nextCreak -= dt
    if (this.nextCreak <= 0) {
      this.creak()
      this.nextCreak = 7 + Math.random() * 12
    }
    this.nextChord -= dt
    if (this.nextChord <= 0 && !this.track) {
      this.chord()
      this.nextChord = 8
    }

    // music thins out as the battery gets low; a soft pulse takes its place
    this.musicGain.gain.setTargetAtTime(this.track ? 0 : 0.11 * (1 - s.low * 0.7), now, 1)
    this.track?.gain.setTargetAtTime(0.35 * (1 - s.low * 0.7), now, 2)
    if (s.low > 0 && Math.floor(this.t * 0.8) !== Math.floor((this.t - dt) * 0.8)) {
      this.tone(220, 0.4, { gain: 0.12 * s.low, dest: this.lowGain })
      this.tone(185, 0.5, { gain: 0.1 * s.low, dest: this.lowGain, when: 0.25 })
      this.lowGain.gain.setTargetAtTime(1, now, 0.1)
    }
  }

  // ---------- one-shots ----------
  step(sprint: boolean) {
    this.burst(0.07, 900 + Math.random() * 500, 1.5, sprint ? 0.12 : 0.08)
    this.tone(70 + Math.random() * 20, 0.08, { gain: 0.06 })
  }
  servo() {
    this.tone(300, 0.18, { type: 'square', gain: 0.02, glide: 1.6 })
  }
  grab() {
    this.tone(660, 0.08, { type: 'triangle', gain: 0.08 })
    this.burst(0.05, 2500, 3, 0.1)
  }
  drop() {
    this.tone(440, 0.1, { type: 'triangle', gain: 0.06, glide: 0.7 })
  }
  throw() {
    this.burst(0.25, 700, 0.8, 0.2)
    this.tone(500, 0.2, { type: 'triangle', gain: 0.05, glide: 0.5 })
  }
  salvage() {
    // prise, clank, rattle
    this.burst(0.12, 300, 2, 0.35)
    for (let i = 0; i < 5; i++) this.tone(800 + Math.random() * 1600, 0.25, { type: 'triangle', gain: 0.06, when: 0.08 + i * 0.06 })
    this.tone(90, 0.4, { gain: 0.2, when: 0.02, glide: 0.6 })
  }
  part() {
    // a little bright chime: you found something you needed
    this.tone(880, 0.6, { gain: 0.1, when: 0.25 })
    this.tone(1318, 0.8, { gain: 0.07, when: 0.35 })
  }
  stow() {
    this.tone(330, 0.12, { type: 'triangle', gain: 0.1 })
    this.tone(495, 0.18, { type: 'triangle', gain: 0.08, when: 0.08 })
  }
  place() {
    this.burst(0.08, 400, 2, 0.3)
    this.tone(120, 0.25, { gain: 0.2, glide: 0.7 })
    this.tone(1200, 0.12, { type: 'triangle', gain: 0.04, when: 0.05 })
  }
  deny() {
    this.tone(180, 0.15, { type: 'square', gain: 0.03 })
    this.tone(150, 0.2, { type: 'square', gain: 0.03, when: 0.1 })
  }
  compact() {
    this.tone(60, 0.5, { gain: 0.3, glide: 0.5 })
    this.burst(0.3, 200, 1, 0.3)
  }
  bale() {
    this.tone(523, 0.4, { gain: 0.08 })
    this.tone(784, 0.6, { gain: 0.06, when: 0.1 })
  }
  powerDown() {
    this.tone(440, 1.8, { type: 'sawtooth', gain: 0.06, glide: 0.15, attack: 0.02 })
    this.tone(220, 2.2, { gain: 0.1, glide: 0.2 })
  }
  powerUp() {
    this.tone(110, 1.2, { gain: 0.1, glide: 4, attack: 0.3 })
    this.tone(660, 0.5, { gain: 0.06, when: 1.0 })
  }
  memory() {
    // a soft, warm bell: something worth remembering
    for (const [f, w] of [[587, 0], [880, 0.15], [740, 0.4]] as const) this.tone(f, 2.5, { gain: 0.09, when: w })
  }
}
