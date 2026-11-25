#include "side.h"
// WP-156 (B4, in instantiated declarations): a static data member, a
// `typeof` operand, a nested-name-specifier, template arguments in a
// specialisation's signature and in a local of an instantiated body, a
// variable template specialisation (`n::S::s` against `Base::s` under
// the rename).
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <int N> struct Q { using t = int; static constexpr int v = N; };
template <class T, unsigned long N> struct arr {};
template <class T> struct j_S { static decltype(T::s()) sm; static __typeof__(T::s()) tm; };
template <class T> decltype(T::s()) j_S<T>::sm = 0;
int j_u1 = j_S<n::S>::sm;
template <class T> typename Q<sizeof(T::s())>::t j_nns(T);
int j_u2 = sizeof(j_nns(n::S()));
template <class T> arr<char, sizeof(T::s())> j_sig(T);
int j_u3 = sizeof(j_sig(n::S()));
template <class T> int j_loc() { arr<char, sizeof(T::s())> a; (void)a; return Q<sizeof(T::s())>::v; }
int j_u4 = j_loc<n::S>();
template <class T> arr<char, sizeof(T::s())> j_vt{};
auto j_u5 = j_vt<n::S>;
