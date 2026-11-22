#include "side.h"
// §5.5 DefinitionBits `0-has-protected-fields`.
#ifdef B
struct C { protected: int a; };
#else
struct C { int a; };
#endif
