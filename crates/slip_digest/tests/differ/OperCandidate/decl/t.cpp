#include "side.h"
// §9.5 OperCandidate `decl`: each candidate of the set is its function.
// Two sources whose sets have one candidate each, a different function.
namespace N { struct Z {}; }
struct W {};
#ifdef B
W operator*(W, W);
template <class T> T x{};
template <class T> auto y = x<T> * x<T>;
N::Z operator*(N::Z, N::Z);
#else
N::Z operator*(N::Z, N::Z);
template <class T> T x{};
template <class T> auto y = x<T> * x<T>;
W operator*(W, W);
#endif
