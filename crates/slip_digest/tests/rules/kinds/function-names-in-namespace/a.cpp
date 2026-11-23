namespace acme {
struct T { int v; };
struct S {
  S();
  ~S();
  operator T() const;
  S &operator=(const S &);
  int operator()(int) const;
};
int operator""_u(unsigned long long);
bool operator==(T, T);
}
int use(acme::S s) { return acme::T(s).v + s(1); }
