# Vermilion

A safe and reliable process manager. The name is derived from the color of paint that is often used as a protective coating on Rust, i.e. this can be used to help protect and run Rust binaries, or any binary.

```
noun: vermilion; noun: vermillion

    a brilliant red pigment made from mercury sulfide (cinnabar).
        a brilliant red color.
        "a lateral stripe of vermilion"

```

## Goals

- Manage and Execute processes
- Make dependencies declaritive
- Capable of replacing init, or being init, in Unix like environments
- Cross platform capable
- Isolation primitives support (containers, sandboxes, jails, etc) depends on platform
- Only safe Rust for all software components
- Simple to use for single applications, but scalable to large graphs of managed software
- Declaritive extensibility
- Async or event driven startup
- Optional dependent restart driven by parent processes
- Log aggregation and forwarding
- Embeddable in other existing init systems
- User mode and super user (priviledged) modes supported

## Non-goals

- Take over the world
- Interprocess communication
- Event bus (This might be in scope)

## Prior art

- [Upstart](http://upstart.ubuntu.com/)
- [systemd](https://www.freedesktop.org/wiki/Software/systemd/)
- [launchd](https://en.wikipedia.org/wiki/Launchd)
- [rc](https://www.freebsd.org/cgi/man.cgi?query=rc&sektion=8&manpath=freebsd-release-ports)
- [daemontools](https://cr.yp.to/daemontools.html)
