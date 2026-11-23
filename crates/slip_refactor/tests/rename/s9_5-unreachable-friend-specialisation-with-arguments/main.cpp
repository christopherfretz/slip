namespace n { template <class T> class X; template <class T> bool eq(const X<T>&, const X<T>&); template <class T> class X { T v{}; friend bool eq<T>(const X&, const X&); }; template <class T> bool eq(const X<T>& a, const X<T>& b) { return a.v == b.v; } }
bool h() { n::X<int> a, b; return n::eq(a, b); }
