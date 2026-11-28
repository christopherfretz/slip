#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
template <class T> auto ga(T) noexcept(true) { return 1; }
template auto ga<int>(int) noexcept(sizeof(make(0)) == 4);
}
