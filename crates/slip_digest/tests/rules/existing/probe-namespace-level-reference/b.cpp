namespace acme { int h(int); int h(double); template <class T> auto f(T t) -> decltype(acme::h(t)); int g() { return 1; } }
