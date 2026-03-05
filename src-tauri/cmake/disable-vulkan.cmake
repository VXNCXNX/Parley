# When CUDA is enabled, force-disable Vulkan to prevent backend conflicts
# in whisper.cpp (both backends try to claim the same tensors).
if(GGML_CUDA)
    set(GGML_VULKAN OFF CACHE BOOL "Disabled: CUDA takes priority" FORCE)
endif()
