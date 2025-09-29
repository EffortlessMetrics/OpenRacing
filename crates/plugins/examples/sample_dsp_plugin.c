/*
 * Sample DSP filter plugin (native C implementation)
 * Implements a simple low-pass filter for force feedback
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#define PLUGIN_ABI_VERSION 1

// Plugin state structure
typedef struct {
    float cutoff_freq;
    float sample_rate;
    float previous_output;
    uint64_t frame_count;
} PluginState;

// Plugin information
typedef struct {
    const char* name;
    const char* version;
    const char* author;
    const char* description;
    uint32_t abi_version;
} PluginInfo;

// Plugin vtable
typedef struct {
    void* (*create)(const uint8_t* config, size_t config_len);
    int (*process)(void* state, float ffb_in, float wheel_speed, float wheel_angle, float dt, float* ffb_out);
    void (*destroy)(void* state);
    PluginInfo (*get_info)(void);
} PluginVTable;

// Create plugin instance
void* plugin_create(const uint8_t* config, size_t config_len) {
    PluginState* state = malloc(sizeof(PluginState));
    if (!state) return NULL;
    
    // Initialize with default values
    state->cutoff_freq = 50.0f;  // 50 Hz cutoff
    state->sample_rate = 1000.0f; // 1 kHz
    state->previous_output = 0.0f;
    state->frame_count = 0;
    
    // Parse config (simplified - real implementation would parse JSON)
    // For now, just use defaults
    
    return state;
}

// Process frame (RT-safe)
int plugin_process(void* state_ptr, float ffb_in, float wheel_speed, float wheel_angle, float dt, float* ffb_out) {
    PluginState* state = (PluginState*)state_ptr;
    if (!state || !ffb_out) return -1;
    
    // Simple low-pass filter
    float rc = 1.0f / (2.0f * M_PI * state->cutoff_freq);
    float alpha = dt / (rc + dt);
    
    float output = alpha * ffb_in + (1.0f - alpha) * state->previous_output;
    state->previous_output = output;
    
    *ffb_out = output;
    state->frame_count++;
    
    return 0; // Success
}

// Destroy plugin instance
void plugin_destroy(void* state) {
    if (state) {
        free(state);
    }
}

// Get plugin information
PluginInfo plugin_get_info(void) {
    PluginInfo info = {
        .name = "Sample DSP Filter",
        .version = "1.0.0",
        .author = "Racing Wheel Suite",
        .description = "Simple low-pass filter for force feedback",
        .abi_version = PLUGIN_ABI_VERSION
    };
    return info;
}

// Export vtable
PluginVTable get_plugin_vtable(void) {
    PluginVTable vtable = {
        .create = plugin_create,
        .process = plugin_process,
        .destroy = plugin_destroy,
        .get_info = plugin_get_info
    };
    return vtable;
}