#include "side.h"
// §5.5 DefinitionBits `0-complete-definition-required` (demand, §11.8): requiring the class complete changes no unit.
#ifdef B
struct C { int a; }; C* p; int n() { return sizeof(C); }
#else
struct C { int a; }; C* p;
#endif
