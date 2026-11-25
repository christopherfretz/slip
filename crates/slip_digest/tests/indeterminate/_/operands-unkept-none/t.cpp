#include "side.h"
// WP-156 fix round 1: what does not fail closed. No constrained
// placeholder in this translation unit has an operand in its arguments
// (`Same<int>`, a literal), so `auto` and a deduced return type are read
// as before; the classes named out of line are named by template-ids with
// literal or type arguments only.
template <class T, class U> concept Same = __is_same(T, U);
template <class T, int N> concept Fits = sizeof(T) <= N;
Same<int> auto v = 1;
Fits<4> auto v2 = 1;
auto w = 2;
auto g() { return 1; }
int h() { return [] { return 3; }(); }
int *n = new int(4);
template <int N> struct Q { static int s; };
template <int N> int Q<N>::s = 1;
template <> int Q<1>::s = 5;
template <class T> struct Info;
struct K {};
template <> struct Info<K> { static unsigned hash(); };
unsigned Info<K>::hash() { return 0; }
