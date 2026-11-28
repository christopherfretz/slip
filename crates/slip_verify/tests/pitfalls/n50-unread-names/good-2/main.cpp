#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
template <class T> auto gb(T) noexcept(true) { return 1; }
extern template auto gb<int>(int) noexcept(sizeof(legacy::make(0)) == 4);
int ub = gb(0);
}
