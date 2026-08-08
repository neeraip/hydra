# Bundled example models

Two models ship with the browser demo so someone without a model of their
own can still run one. Both come from their engine's upstream project, both
are MIT-licensed, and both are **unmodified** — a bundled example that has
been quietly edited is worse than no example, because it invites comparison
against a file that does not exist anywhere else.

The directory is `models/` rather than `examples/` because Cargo reserves
that name for example binaries.

## These are not test fixtures

`tests/fixtures/` holds small models written to isolate one behaviour, and
nothing real is vendored there on purpose. These are the opposite: real
models, kept whole, for a human to look at. Do not use them in tests — a
test that depends on a third party's example is a test that changes meaning
when they revise it.

## Net1.inp

EPANET's Example Network 1: a small distribution network modelling chlorine
decay, with both bulk and wall reactions, over a 24-hour run.

From <https://github.com/OpenWaterAnalytics/EPANET>, `example-networks/Net1.inp`.

> MIT License
>
> Copyright (c) 2019 (See AUTHORS)
>
> Permission is hereby granted, free of charge, to any person obtaining a
> copy of this software and associated documentation files (the "Software"),
> to deal in the Software without restriction, including without limitation
> the rights to use, copy, modify, merge, publish, distribute, sublicense,
> and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice, list of authors, and this permission notice
> shall be included in all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
> THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.

## Simulation1.inp

SWMM's Example 1 drainage network, dynamic-wave routing with Horton
infiltration, over a 36-hour run.

From <https://github.com/OpenWaterAnalytics/Stormwater-Management-Model>,
`tests/solver/data/hotstart/Simulation1.inp`. It lives in that project's
hotstart test data, and it declares `SAVE HOTSTART` — which is why the demo
offers the hotstart file as a download after the run rather than reporting
that it cannot be written.

> MIT License
>
> Copyright (c) 2020 Open Water Analytics
>
> Permission is hereby granted, free of charge, to any person obtaining a
> copy of this software and associated documentation files (the "Software"),
> to deal in the Software without restriction, including without limitation
> the rights to use, copy, modify, merge, publish, distribute, sublicense,
> and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
> THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.
