#include "side.h"
// §5.5 DefinitionBits `0-has-inherited-ctor`.
#ifdef B
struct P { P(int); }; struct C : P { using P::P; };
#else
struct P { P(int); }; struct C : P { };
#endif
