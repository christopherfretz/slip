#include "side.h"
// §5.5 DefinitionBits `0-has-private-fields`.
#ifdef B
struct C { private: int a; };
#else
struct C { int a; };
#endif
