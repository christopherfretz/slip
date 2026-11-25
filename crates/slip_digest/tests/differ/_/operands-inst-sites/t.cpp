#include "side.h"
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <class T, unsigned long N> struct arr {};
template <class T> auto i_ret(T) -> decltype(T::s());
int i_u1 = sizeof(i_ret(n::S()));
template <class T> void i_par(T, char (*)[sizeof(T::s())] = nullptr);
void i_u2() { i_par(n::S()); }
template <class T> auto i_lost(T) -> decltype(T::s());
int i_lost(n::S);
int i_u3 = sizeof(i_lost(n::S()));
template <class T> struct i_Ca { int a[sizeof(T::s())]; };
i_Ca<n::S> i_ca;
template <class T> struct i_Cf { decltype(T::s()) f; };
i_Cf<n::S> i_cf;
template <class T> struct i_Cg { arr<char, sizeof(T::s())> g; };
i_Cg<n::S> i_cg;
template <class T> struct i_Ct { typedef decltype(T::s()) type; };
i_Ct<n::S>::type i_ct;
template <class T> struct i_Cu { using al = decltype(T::s()); };
i_Cu<n::S>::al i_cu;
template <class T> struct i_Cm { decltype(T::s()) mf(); };
i_Cm<n::S> i_cm;
template <class T> struct i_Ce { explicit(sizeof(T::s()) == 4) i_Ce(int); };
i_Ce<n::S> i_ce(1);
template <class T> struct i_Cn { void ne() noexcept(noexcept(T::s())); };
i_Cn<n::S> i_cn;
bool i_cnu = noexcept(i_cn.ne());
template <class T> struct i_Cb { int bf : sizeof(T::s()); };
i_Cb<n::S> i_cb;
template <class T> struct i_D : arr<char, sizeof(T::s())> {};
i_D<n::S> i_d;
template <class T> int i_body() { decltype(T::s()) l = 0; return l; }
int i_u4 = i_body<n::S>();
template <class T> int i_body2() { char b[sizeof(T::s())]; return sizeof(b); }
int i_u4b = i_body2<n::S>();
template <class T> int i_body3() { return static_cast<decltype(T::s())>(0); }
int i_u4c = i_body3<n::S>();
template <class T> decltype(T::s()) i_vt = 0;
int i_u5 = i_vt<n::S>;
template <class T> void i_ne(T) noexcept(noexcept(T::s())) {}
bool i_u7 = noexcept(i_ne(n::S()));
struct i_W { template <class T> explicit(sizeof(T::s()) != sizeof(int)) i_W(T) {} };
i_W i_w{n::S()};
template <class T> using i_R = decltype(T::s());
i_R<n::S> i_v;
template <class T> struct i_M { template <class U> auto g(U u) -> decltype(T::s(), u); };
i_M<n::S> i_m;
template <class T> struct i_M2 { template <class U> void k(U, decltype(T::s())); };
i_M2<n::S> i_m2;
