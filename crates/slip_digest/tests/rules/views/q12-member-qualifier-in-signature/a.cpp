#include "new_dep.h"
namespace legacy { struct B { int f(); }; }
template <class T> auto f(T t) -> decltype(t.legacy::B::f());
