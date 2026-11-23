// std: c++26
#include "side.h"
// digest-kinds rule §3.3: a name-independent member names no unnamed class either; the ordinal does.
struct S {
#ifdef B
struct { long _; } _; struct { int _; } _;
#else
struct { int _; } _; struct { long _; } _;
#endif
};
