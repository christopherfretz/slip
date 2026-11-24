#include "dep.h"
namespace legacy { struct B { int f(); }; }
struct WB { int f(); };
struct W : legacy::B, WB { struct acme { using B = WB; }; struct legacy2 {}; };
template <class T> int f(T t) { return t.legacy::B::f(); }
int x = f(W());
