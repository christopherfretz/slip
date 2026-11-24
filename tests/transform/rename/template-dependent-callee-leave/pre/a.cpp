namespace a {
int f(int x) { return x; }
template <class T> int call(T t) { return f(t); }
}
int u() { return a::call(1) + a::call(2L); }
