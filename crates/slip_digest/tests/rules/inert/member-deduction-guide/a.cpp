namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct O {
  template <class U> struct I { I(U); };
  template <class U> I(U) -> I<U>;
  static int g() { I i(2); return 0; }
};
int f() { return O<int>::g(); }
