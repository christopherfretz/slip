#include "side.h"
// §5.5 DefinitionBits `0-defaulted-default-ctor-deleted`: Kiln sets the bit
// only for an implicit default constructor it declares at `}`, which an
// inherited default constructor makes it do.
#ifdef B
struct P { P(); }; struct C : P { using P::P; int& r; };
#else
struct P { P(); }; struct C : P { using P::P; int r; };
#endif
