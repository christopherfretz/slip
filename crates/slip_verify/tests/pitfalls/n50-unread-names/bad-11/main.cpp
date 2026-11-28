#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
template <int N> int at() { return N; }
template int at<sizeof(make(0))>();
}
