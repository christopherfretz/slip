// std: c++20
#include "side.h"
// digest §4.3: a closure in an enumerator's initialiser has no owner with
// a stream that numbers it (digest-kinds §10.4): `local/closure-owner`.
enum E { a = [] { return 1; }() };
