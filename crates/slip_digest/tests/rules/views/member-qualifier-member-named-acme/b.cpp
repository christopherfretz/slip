#include "dep.h"
namespace acme { struct B { int f(); }; }
struct WB { int f(); };
struct W : acme::B, WB { struct acme { using B = WB; }; struct legacy2 {}; };
template <class T> int f(T t) { return t.acme::B::f(); }
int x = f(W());
