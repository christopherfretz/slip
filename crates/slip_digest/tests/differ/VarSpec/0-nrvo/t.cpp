#include "side.h"
// §5.4 VarSpec `0-nrvo`: a local variable returned by name.
#ifdef B
struct S { S(); S(const S&); };
S f() { S s; return s; }
#else
struct S { S(); S(const S&); };
S f() { S s; S t; return t; }
#endif
