#include "side.h"
// std: c++26
// §4.3 BuiltinType `MetaInfo`: against `std::nullptr_t`.
#ifdef B
using v = decltype(nullptr);
#else
using v = decltype(^^int);
#endif
