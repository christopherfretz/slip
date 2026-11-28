#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
template <class T> decltype(auto) gc(T) noexcept(true) { return 1; }
template decltype(auto) gc<int>(int) noexcept(sizeof(make(0)) == 4);
}
