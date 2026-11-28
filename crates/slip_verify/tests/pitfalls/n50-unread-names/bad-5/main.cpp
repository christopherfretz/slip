#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
template <class T> void ftn(T) noexcept(true) {}
struct FN { friend void ftn<>(int) noexcept(sizeof(make(0)) == 4); };
}
