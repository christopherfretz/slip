#include "dep.h"
namespace acme { struct B { int f(); }; }
template <class T> int f(T t) { return t.acme::B::f(); }
