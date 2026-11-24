#include "dep.h"
namespace legacy { struct B { int f(); }; }
template <class T> int f(T t) { return t.legacy::B::f(); }
