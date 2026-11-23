#include "side.h"
// digest §4.3 "Lambda-expressions inside a key or a type": a lambda in a
// dependent `decltype` is walked inside a type digest.
template <class T> struct Y { decltype([] { return T(); }()) m; };
