import { tool_definitions } from './pkg/chirrp.js';

/**
 * Bind one callable per tool to a engine instance. Call after WASM init().
 * Each entry has {name, description, input_schema, execute(arguments)}.
 * Supply publishAudio to return an attachment/URL to the model instead of PCM.
 * Host functions run locally; no provider SDK or network access is required.
 */
export function createChirrpTools(engine, { publishAudio } = {}) {
  const parse = (json) => JSON.parse(json);
  const handlers = {
    random_sound: ({ seed = 42, population = 6 }) => parse(engine.random_sound(seed, population)),
    list_sounds: () => parse(engine.list_sounds()),
    create_sound: ({ kind, seed = 42, population = 6 }) => parse(engine.create_sound(kind, seed, population)),
    list_candidates: () => parse(engine.list_candidates()),
    select_candidate: ({ index }) => parse(engine.select_candidate(index)),
    randomize: ({ strength = 0.45, seed = 42 }) => parse(engine.randomize(strength, seed)),
    evolve: ({ ratings, strength = 0.45, seed = 42 }) => parse(engine.evolve(ratings, strength, seed)),
    edit_sound: (edits) => parse(engine.edit_sound(JSON.stringify(edits))),
    get_recipe: ({ index }) => parse(engine.get_recipe(index)),
    analyze: ({ index, sample_rate = 48000 }) => parse(engine.analyze(index, sample_rate)),
    render_audio: ({ index, sample_rate = 48000 }) => audioResult('pcm_f32', engine.render_audio(index, sample_rate), sample_rate),
    export_wav: ({ index, sample_rate = 48000 }) => audioResult('wav', engine.export_wav(index, sample_rate), sample_rate),
  };
  function audioResult(format, data, sample_rate) {
    const result = { format, channels: 2, sample_rate, data };
    return publishAudio ? publishAudio(result) : result;
  }
  return Object.fromEntries(JSON.parse(tool_definitions()).map(definition => {
    const handler = handlers[definition.name];
    if (!handler) throw new Error(`Missing tool handler: ${definition.name}`);
    return [definition.name, {
      ...definition,
      execute: (args = {}) => {
        // Validate before the WASM ABI can coerce fractions/negative numbers
        // into unsigned integers, or JSON.stringify can turn NaN into null.
        validate(definition.input_schema, args, definition.name);
        return handler(args);
      },
    }];
  }));
}

// Validator for the deliberately small schema vocabulary used by this registry.
function validate(schema, value, path) {
  if (schema.type === 'object') {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${path} must be an object`);
    for (const key of schema.required ?? []) if (!Object.hasOwn(value, key)) throw new Error(`${path}.${key} is required`);
    if (Object.keys(value).length < (schema.minProperties ?? 0)) throw new Error(`${path} requires at least one control`);
    for (const [key, item] of Object.entries(value)) {
      if (!Object.hasOwn(schema.properties, key)) throw new Error(`Unknown argument: ${path}.${key}`);
      validate(schema.properties[key], item, `${path}.${key}`);
    }
  } else if (schema.type === 'array') {
    if (!Array.isArray(value) || value.length < schema.minItems || value.length > schema.maxItems) throw new Error(`${path} has an invalid array length`);
    value.forEach((item, i) => validate(schema.items, item, `${path}[${i}]`));
  } else if (schema.type === 'number' || schema.type === 'integer') {
    if (typeof value !== 'number' || !Number.isFinite(value) || (schema.type === 'integer' && !Number.isInteger(value)) || value < schema.minimum || value > schema.maximum) {
      throw new Error(`${path} must be a ${schema.type} in [${schema.minimum}, ${schema.maximum}]`);
    }
  } else if (schema.type === 'string') {
    if (typeof value !== 'string' || (schema.enum && !schema.enum.includes(value))) throw new Error(`${path} must be one of: ${schema.enum?.join(', ')}`);
  } else { throw new Error(`Unsupported schema type: ${schema.type}`); }
}
