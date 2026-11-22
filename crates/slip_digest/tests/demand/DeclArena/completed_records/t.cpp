#include "side.h"
// §5.7 DeclArena `completed_records` (demand, §11): a use that completes an instantiation leaves every unit equal.
template <class T> struct A { T t; };
#ifdef B
int y() { A<int> a{}; return a.t; }
#endif
