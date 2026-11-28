#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
template <class T> struct GM { auto f() noexcept(true) { return 1; } };
template auto GM<int>::f() noexcept(sizeof(make(0)) == 4);
}
