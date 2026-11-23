// std: c++26
namespace acme {
struct S { int x; };
consteval { (void)sizeof(S); }
}
consteval { (void)sizeof(acme::S); }
