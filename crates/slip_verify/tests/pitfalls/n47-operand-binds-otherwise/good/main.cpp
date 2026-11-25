#include "dep.h"

template <class T> int make(T);

namespace acme {
decltype(legacy::make(0)) v = 0;
}
