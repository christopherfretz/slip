namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct B { T v; B(T x); operator T() const { return v; } };
template <class T> struct D : B<T> { using B<T>::B; using B<T>::operator T; };
int f() { D<int> d(1); return d; }
