#include "dep.h"

template <class T> int make(T);
template <int N> struct A {};

namespace acme {
A<sizeof(make(0))> a;
}
