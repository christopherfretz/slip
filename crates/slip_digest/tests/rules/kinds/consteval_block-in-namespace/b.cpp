// std: c++26
namespace legacy {
struct S { int x; };
consteval { (void)sizeof(S); }
}
consteval { (void)sizeof(legacy::S); }
