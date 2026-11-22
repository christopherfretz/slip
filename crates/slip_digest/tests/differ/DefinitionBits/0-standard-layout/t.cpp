#include "side.h"
// §5.5 DefinitionBits `0-standard-layout`.
#ifdef B
struct C { int a; private: int b; };
#else
struct C { int a; int b; };
#endif
