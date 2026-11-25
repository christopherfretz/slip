#include "side.h"
// WP-155: what is constant-initialised and trivially destroyed is not
// emitted unreferenced: a constant inline variable, a constexpr or const
// integral variable, a zero-initialised one, a static member of a class
// template instantiated with a constant initialiser.
inline constexpr int k = 3;
constexpr int c = 2;
const int ci = 4;
int z;
template <class T> struct C { static constexpr int v = sizeof(T); static const int w; };
template <class T> const int C<T>::w = 5;
static_assert(C<int>::v == 4 && C<int>::w == 5);
