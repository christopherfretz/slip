#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
template <class T, class U> struct Q3 { static int s; };
template <class T, class U> int Q3<T, U>::s = 0;
template <> int Q3<int, int>::s = 1;
int f13() { return Q3<int, decltype(legacy::make(0))>::s; }
}
