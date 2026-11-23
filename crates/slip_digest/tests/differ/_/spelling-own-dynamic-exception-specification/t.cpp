// std: c++17
// args: -Wno-dynamic-exception-spec
#include "side.h"
// digest-kinds §4.2 rule 3: a function's own dynamic exception specification is written with its types in C++17 (the reviewer's u1).
void h();
#ifdef B
void g() throw(long) { h(); }
#else
void g() throw(int) { h(); }
#endif
