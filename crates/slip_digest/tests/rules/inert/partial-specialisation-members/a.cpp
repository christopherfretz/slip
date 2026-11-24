namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W;
template <class T> struct W<T*> { T* p; operator T*() const { return p; } int g() const { return 1; } W(); ~W() {} bool operator!() const { return !p; } };
int f(W<int*> w) { int *q = w; return w.g() + !w + (q != nullptr); }
