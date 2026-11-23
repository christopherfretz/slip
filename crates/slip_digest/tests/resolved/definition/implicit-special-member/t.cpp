#include "side.h"
// digest-kinds §11.9: an implicitly declared copy constructor is defined
// on odr-use, in `definition`.
struct S { int a; S(); };
#ifdef B
S h(const S& s) { return s; }
#endif
