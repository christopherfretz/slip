namespace acme { enum E { A, B }; }
template <auto N> struct V { static constexpr auto n = N; };
V<acme::B> v;
int use() { return V<acme::A>::n; }
