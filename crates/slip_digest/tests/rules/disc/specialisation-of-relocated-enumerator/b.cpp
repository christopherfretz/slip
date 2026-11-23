namespace legacy { enum E { A, B }; }
template <auto N> struct V { static constexpr auto n = N; };
V<legacy::B> v;
int use() { return V<legacy::A>::n; }
