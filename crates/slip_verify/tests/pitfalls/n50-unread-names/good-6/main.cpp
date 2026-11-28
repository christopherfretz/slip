#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
#define CAST(w, v) ((_BitInt(w))(v))
int c37() { return (int)CAST(sizeof(legacy::make(0)) * 8, 1); }
}
