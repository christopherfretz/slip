#include "side.h"
// digest §4.3 "Lambda-expressions inside a key or a type": a lambda in a
// function template's signature is walked inside its key.
template <class T> void f(decltype([](T) {}) x);
