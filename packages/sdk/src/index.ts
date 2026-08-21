/**
 * Frontend adapter for the declared-event package boundary.
 * The desktop shell supplies the transport; this adapter never imports runtime
 * internals or exposes filesystem/updater operations.
 */
export type PackageEvent = {
  name: string;
  payload: unknown;
};

export type PackageEventTransport = {
  emit(event: PackageEvent): Promise<void>;
  listen(eventName: string, handler: (event: PackageEvent) => void): () => void;
};

export class PackageSdk {
  public constructor(private readonly transport: PackageEventTransport) {}

  public emit(name: string, payload: unknown): Promise<void> {
    return this.transport.emit({ name, payload });
  }

  public listen(
    eventName: string,
    handler: (event: PackageEvent) => void,
  ): () => void {
    return this.transport.listen(eventName, handler);
  }
}
