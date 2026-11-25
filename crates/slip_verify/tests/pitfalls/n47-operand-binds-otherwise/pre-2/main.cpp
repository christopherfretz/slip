#include "dep.h"

template <class T> int make(T);
template <int N> struct A {};

namespace legacy {
A<sizeof(make(0))> a;
}
