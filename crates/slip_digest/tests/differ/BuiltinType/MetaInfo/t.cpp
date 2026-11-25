#include "side.h"
// std: c++26
// §4.3 BuiltinType `MetaInfo`: against `std::nullptr_t`. (WP-156: the
// reflection is named once through `info` on both sides, so that the
// alias `v` holds no operand: `info`'s operand `^^int` is unsupported,
// and its operand unit indeterminate on both sides.)
using info = decltype(^^int);
#ifdef B
using v = decltype(nullptr);
#else
using v = info;
#endif
