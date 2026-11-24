#include "new_dep.h"
namespace acme { struct B { int f(); }; }
template <class T> auto f(T t) -> decltype(t.acme::B::f());
