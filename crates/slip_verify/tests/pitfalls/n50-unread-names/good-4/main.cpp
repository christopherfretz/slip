#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
template <class T> struct GM { auto f() noexcept(true) { return 1; } };
template auto GM<int>::f() noexcept(sizeof(legacy::make(0)) == 4);
}
