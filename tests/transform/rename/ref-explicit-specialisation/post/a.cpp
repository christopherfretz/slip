namespace a {
template <class T> T g(T x) { return x; }
template <> int g<int>(int x) { return x + 1; }
template long g<long>(long);
template short g(short);
}
int u() { return a::g(1); }
