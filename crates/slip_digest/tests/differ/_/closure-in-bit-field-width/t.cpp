// std: c++20
#include "side.h"
// digest-kinds §10.4: a lambda in a bit-field width is owned by the field,
// whose stream walks the width.
#ifdef B
struct S { int a : [] { return 4; }(); };
#else
struct S { int a : [] { return 3; }(); };
#endif
