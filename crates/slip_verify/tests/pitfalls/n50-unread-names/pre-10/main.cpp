#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
void d10() { Wr<int> wv{0}; wv.~decltype(Wr<int>{make(0)})(); }
}
