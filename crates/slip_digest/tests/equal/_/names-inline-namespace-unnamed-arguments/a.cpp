// WP-104 review F7: inline namespaces and unnamed classes in names and arguments (keys_and_names_are_unique); closures as
// arguments collide by key (known, the local package's) and are left out
template <class T> struct store { void put(T, int = 0) {} };
template <class... T> struct many {};
namespace legacy {
struct Mine {}; using M1 = Mine;
inline namespace v2 { struct Mine {}; }
struct { int a; } u1; struct { int b; } u2;
template <class T> struct box { struct inner {}; };
template <class T> void take(T) {}
enum { A0 } e1; enum { B0 } e2;
}
store<legacy::M1> s1; store<legacy::v2::Mine> s2;
store<decltype(legacy::u1)> s5; store<decltype(legacy::u2)> s6;
store<void (*)(legacy::M1)> s7; store<int legacy::M1::*> s8;
store<legacy::box<legacy::M1>::inner> s9; store<legacy::box<legacy::v2::Mine>::inner> s10;
store<decltype(legacy::e1)> s11; store<decltype(legacy::e2)> s12;
many<legacy::M1, legacy::v2::Mine> m1;
void use() { legacy::take(legacy::M1{}); legacy::take(legacy::v2::Mine{});
  s1.put({}); s2.put({}); }
