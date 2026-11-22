#include "side.h"
// digest §2.2, digest-kinds §9.5 (WP-032 rule 3): a member-like friend
// ([temp.friend]/9) is keyed in the class specialisation that declares it,
// so `M<int>`'s and `M<long>`'s are two units, which do not collide. Side B
// changes the parameter of `N`'s friend only (its constraint is an
// expression, pending today): its pattern's friend moves, and so does the
// friend `N<int>` declares; `M`'s do not (the unit test
// `units::tests::member_like_friends_move_alone` checks that by key).
template <class T> concept C = true;
template <class T> struct M { template <class U> friend void m(U) requires C<T> {} };
template <class T> struct N {
#ifdef B
  template <class U> friend void m(U *) requires C<T> {}
#else
  template <class U> friend void m(U) requires C<T> {}
#endif
};
M<int> a;
M<long> b;
N<int> c;
