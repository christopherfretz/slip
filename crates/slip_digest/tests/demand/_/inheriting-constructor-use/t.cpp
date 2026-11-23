#include "side.h"
// digest §14 (WP-045 review probe): an inheriting constructor, used.
struct B0 { B0(int); };
template <class T> struct D : B0 { using B0::B0; };
D<int>* p;
#ifdef B
void use() { D<int> d(1); }
#endif
