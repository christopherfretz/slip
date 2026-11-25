#include "side.h"
// WP-156 fix round 1: what does not fail closed. No constrained
// placeholder in this translation unit has an operand in its arguments
// (`Same<int>`, a literal), so `auto` and a deduced return type are read
// as before; the classes named out of line are named by template-ids with
// type arguments only, none a type of the arena that holds an operand.
// A non-type argument written as a literal fails nothing either
// (`Q2<1>::s`: fix round 2, no expression that is no literal before the
// name); one written with an expression fails closed (Kiln keeps its
// value alone).
template <class T, class U> concept Same = __is_same(T, U);
template <class T, int N> concept Fits = sizeof(T) <= N;
Same<int> auto v = 1;
Fits<4> auto v2 = 1;
auto w = 2;
auto g() { return 1; }
int h() { return [] { return 3; }(); }
int *n = new int(4);
template <class T> struct Q { static int s; };
template <class T> int Q<T>::s = 1;
template <> int Q<char>::s = 5;
template <class T> struct Info;
struct K {};
template <> struct Info<K> { static unsigned hash(); };
unsigned Info<K>::hash() { return 0; }
template <int N> struct Q2 { static int s; void f(); };
template <> struct Q2<1> { static int s; void f(); };
int Q2<1>::s = 2;
void Q2<1>::f() {}
// Fix round 2: an explicit instantiation that writes a literal
// `noexcept(true)` holds no operand (no unit fails).
template <class T> void gl(T) noexcept(true) {}
template void gl<int>(int) noexcept(true);
