# Parakeet

A mobile messenger that sends over **Matrix** protocol by default and falls back to **SMS** when the
homeserver cannot be reached — the way RCS degrades — without the recipient ever seeing the same
message twice.

## The problem

One logical message can travel two independent pipes. If the sender is offline it goes out as an
SMS; when they get data back the same message is put on Matrix so both parties get the real event.
The recipient now holds two copies of one message.

[//]: # (TODO: Detail landing and specs)