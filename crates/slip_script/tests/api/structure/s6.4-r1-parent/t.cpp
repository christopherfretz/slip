namespace n {
struct S {
  int m;
  enum E { A };
  struct In {};
};
enum class C { B };
void f(int p);
inline namespace i {
int x;
}
namespace {
int y;
}
struct F {
  friend void g(F);
};
}
