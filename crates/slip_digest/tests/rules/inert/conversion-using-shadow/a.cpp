namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct B { T v; operator T() const { return v; } };
template <class T> struct D : B<T> { using B<T>::operator T; };
int f(D<int> d) { return d; }
