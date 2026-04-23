Ground is a systems design language. Infrastructure is derived.

Always start a session with reading the GROUND-BOOK.md and README.md .

## Current focus

Find a ground for std library. Later it will part of the compiler crate, but in order to quickly iterate now we develop it in ../ground-test/ test project.
In the future it will be just a preregistered set of units in the compiler. Now we can model same behaviour by creating std/ folder in the test project.
As we currently at the very beginning use this way of creating packs, e.g. std/aws.grd, std/aws/tf.grd not std/aws/pack.grd .
Strategy is simple, take 1 aws tf entity, implement it on the lowest layer 1 to 1 (std:aws:tf) with Typescript and Tera templates included.

## Other notes

> [IMPORTANT!] be concise

./devspec/ is purely historical, only use GROUND-BOOK.md & README.md for reference.
The best source of truth is ./src.
Keep code formatted, call format after patch
