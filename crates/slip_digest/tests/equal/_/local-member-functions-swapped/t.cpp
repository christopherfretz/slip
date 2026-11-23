#include "side.h"
// §4.3, §5.5 `members-other`: a local class's member functions are written by key: their order is not behaviour.
#ifdef B
void f() { struct L { int h() { return 2; } int g() { return 1; } }; }
#else
void f() { struct L { int g() { return 1; } int h() { return 2; } }; }
#endif
