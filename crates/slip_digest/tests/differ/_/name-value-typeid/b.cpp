// WP-104 (digest §9), consumed: typeid(__func__)
namespace std { class type_info; }
const std::type_info &start(int) { return typeid(__func__); }
