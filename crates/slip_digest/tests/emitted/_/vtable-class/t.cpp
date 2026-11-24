#include "side.h"
// WP-155: a class with virtual functions has its vtable and RTTI emitted
// where its key function is defined; none is a unit's definition, and
// its functions are not emitted unreferenced.
struct V { virtual ~V(); virtual int f(); };
V::~V() {}
int V::f() { return 1; }
template <class T> struct W { virtual int g() { return 2; } };
W<int> *w;
int h() { return W<int>().g(); }
