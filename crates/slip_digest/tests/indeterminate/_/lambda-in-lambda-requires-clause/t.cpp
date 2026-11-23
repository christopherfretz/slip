// std: c++20
#include "side.h"
// digest §4.3: a lambda in the trailing requires-clause of a generic
// lambda, in a body: the clause is part of the call operator's key, so
// the lambda is in a key (`local/lambda-in-key`), in f's stream that
// writes the call operator in place and in the specialisation's unit.
int f() { return []<class T>(T a) requires ([] { return sizeof(T) > 0; }()) { return a; }(1); }
