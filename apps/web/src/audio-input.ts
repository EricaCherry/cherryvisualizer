type ConnectFn = (ctx: AudioContext, source: AudioNode) => void;

/**
 * Web Audio plumbing for the app. Owns the AudioContext and the available
 * sources (a looping <audio> element for files, a mic stream), and hands the
 * active source to the bus via `onConnect`. The mic is intentionally NOT routed
 * to the speakers to avoid feedback.
 */
export class AudioInput {
  ctx: AudioContext | null = null;
  onConnect?: ConnectFn;
  onStatus?: (status: string) => void;
  /** Supplies the shared AudioContext (set by the host so Butterchurn etc. share it). */
  acquireContext?: () => AudioContext;

  private audioEl: HTMLAudioElement | null = null;
  private elSource: MediaElementAudioSourceNode | null = null;
  private micStream: MediaStream | null = null;
  private objectUrl: string | null = null;

  private ensureCtx(): AudioContext {
    if (!this.ctx) {
      this.ctx =
        this.acquireContext?.() ??
        new (window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
    }
    return this.ctx;
  }

  private ensureElement(ctx: AudioContext): HTMLAudioElement {
    if (!this.audioEl) {
      this.audioEl = new Audio();
      this.audioEl.loop = true;
      // a MediaElementSource can only be created once per element
      this.elSource = ctx.createMediaElementSource(this.audioEl);
      this.elSource.connect(ctx.destination);
    }
    return this.audioEl;
  }

  async playFile(file: File): Promise<void> {
    const ctx = this.ensureCtx();
    await ctx.resume();
    const el = this.ensureElement(ctx);
    this.stopMic();
    if (this.objectUrl) URL.revokeObjectURL(this.objectUrl);
    this.objectUrl = URL.createObjectURL(file);
    el.src = this.objectUrl;
    this.onConnect?.(ctx, this.elSource!);
    await el.play();
    this.onStatus?.(`▶ ${file.name}`);
  }

  async useMic(): Promise<void> {
    const ctx = this.ensureCtx();
    await ctx.resume();
    this.audioEl?.pause();
    this.micStream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false },
    });
    const src = ctx.createMediaStreamSource(this.micStream);
    this.onConnect?.(ctx, src);
    this.onStatus?.('🎤 microphone');
  }

  togglePlay(): boolean {
    if (!this.audioEl) return false;
    if (this.audioEl.paused) {
      void this.audioEl.play();
      return true;
    }
    this.audioEl.pause();
    return false;
  }

  private stopMic(): void {
    if (this.micStream) {
      this.micStream.getTracks().forEach((t) => t.stop());
      this.micStream = null;
    }
  }
}
