#pragma once

#include <cstddef>

void* pneuma_alloc(std::size_t size);
void pneuma_free(void* ptr);
