namespace a { struct S { int f() { return 1; } }; }
template <class T> int probe(T t) {
  if constexpr (requires { t.f(); })
    return 1;
  else
    return 0;
}
int u() { return probe(a::S{}) + a::S{}.f(); }
