#include "side.h"
// §9.5 OperLookups `by_expr`: the set of a dependent operator expression,
// by its candidates. One operator found against two.
namespace N { struct Z {}; }
struct W {};
N::Z operator-(N::Z, N::Z);
#ifdef B
W operator-(W, W);
template <class T> T x{};
template <class T> auto y = x<T> - x<T>;
#else
template <class T> T x{};
template <class T> auto y = x<T> - x<T>;
W operator-(W, W);
#endif
