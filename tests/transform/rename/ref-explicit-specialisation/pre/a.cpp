namespace a {
template <class T> T f(T x) { return x; }
template <> int f<int>(int x) { return x + 1; }
template long f<long>(long);
template short f(short);
}
int u() { return a::f(1); }
