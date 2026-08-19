use crate::{LOGICAL_FIXTURE_VERSION, LogicalNetlist, PCB_CANDIDATE_SCHEMA_VERSION};
use jingwei_geometry::{Angle, BoundingBox, Length, Point, Polygon, Polyline, Transform, Vector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// PCB domain construction or invariant error.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PcbError {
    /// The logical fixture uses an unsupported version.
    #[error("unsupported logical netlist fixture version: {0}")]
    UnsupportedLogicalVersion(String),
    /// A component lacks an exact footprint.
    #[error("component {0} has no exact footprint binding")]
    MissingFootprint(String),
    /// A logical pin has no exact pad mapping.
    #[error("component {component_id} pin {pin_number} has no pad mapping")]
    MissingPadMapping {
        /// Component ID.
        component_id: String,
        /// Pin number.
        pin_number: String,
    },
    /// A pad mapping references a nonexistent footprint pad.
    #[error("component {component_id} maps pin {pin_number} to unknown pad {pad_number}")]
    UnknownPad {
        /// Component ID.
        component_id: String,
        /// Pin number.
        pin_number: String,
        /// Referenced pad number.
        pad_number: String,
    },
    /// A pin references an unknown net.
    #[error("pin {pin_id} references unknown net {net_id}")]
    UnknownNet {
        /// Pin ID.
        pin_id: String,
        /// Net ID.
        net_id: String,
    },
    /// A board outline is invalid.
    #[error("board outline is invalid: {0}")]
    InvalidOutline(String),
    /// An edit references an unknown entity.
    #[error("unknown entity {0}")]
    UnknownEntity(String),
    /// A locked entity would be modified.
    #[error("entity {0} is locked")]
    LockedEntity(String),
    /// A semantic edit is malformed.
    #[error("invalid edit: {0}")]
    InvalidEdit(String),
}

/// Copper or non-copper board layer kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerKind {
    /// Copper routing layer.
    Copper,
    /// Solder mask.
    SolderMask,
    /// Silkscreen.
    Silkscreen,
    /// Board outline.
    EdgeCuts,
    /// Fabrication drawing.
    Fabrication,
    /// Courtyard.
    Courtyard,
}

/// One physical stackup layer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopperLayer {
    /// Stable layer ID.
    pub id: String,
    /// Reference-backend layer name.
    pub name: String,
    /// Layer kind.
    pub kind: LayerKind,
    /// Copper thickness for copper layers.
    pub copper_thickness: Length,
}

/// Board stackup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stackup {
    /// Finished board thickness.
    pub board_thickness: Length,
    /// Ordered layers.
    pub layers: Vec<CopperLayer>,
    /// Surface finish.
    pub surface_finish: String,
}

/// A drilled mechanical feature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanicalFeature {
    /// Stable entity ID.
    pub id: String,
    /// Feature center.
    pub center: Point,
    /// Drill or slot width.
    pub drill: Length,
    /// Optional slot length.
    pub slot_length: Length,
    /// Whether the feature is plated.
    pub plated: bool,
    /// Whether placement and edits must preserve this feature.
    pub locked: bool,
}

/// Board region used for keepout, placement or height constraints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardRegion {
    /// Stable region ID.
    pub id: String,
    /// Region kind.
    pub kind: String,
    /// Region boundary.
    pub boundary: Polygon,
    /// Optional maximum component height.
    pub maximum_height: Option<Length>,
}

/// Board mechanics and stackup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardDefinition {
    /// The one authoritative board outline.
    pub outline: Polygon,
    /// Board cutouts.
    pub cutouts: Vec<Polygon>,
    /// Holes and slots.
    pub mounting_features: Vec<MechanicalFeature>,
    /// Stackup.
    pub stackup: Stackup,
    /// Placement, routing and height regions.
    pub regions: Vec<BoardRegion>,
    /// Absolute coordinate origin.
    pub origin: Point,
}

/// Front or back assembly side.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// Top side.
    Front,
    /// Bottom side.
    Back,
}

/// Policy controlling AI and solver edits.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditControl {
    /// Automated edits are allowed.
    #[default]
    AiEditable,
    /// Automated edits require explicit review.
    AskBeforeChange,
    /// All edits require an explicit unlock operation.
    Locked,
}

/// Placed or unplaced footprint candidate.
#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Footprint {
    /// Stable PCB entity ID.
    pub id: String,
    /// Upstream logical component ID.
    pub logical_component_id: String,
    /// Reference designator.
    pub designator: String,
    /// Value.
    pub value: String,
    /// Exact manufacturer.
    pub manufacturer: String,
    /// Exact MPN.
    pub mpn: String,
    /// Exact footprint name.
    pub footprint_name: String,
    /// Placement role.
    pub placement_role: String,
    /// Current center, or none before placement.
    pub position: Option<Point>,
    /// Orthogonal Phase 0 rotation.
    pub rotation: Angle,
    /// Assembly side.
    pub side: Side,
    /// Nominal body size.
    pub body_size: (Length, Length),
    /// Courtyard expansion.
    pub courtyard_margin: Length,
    /// Component height.
    pub height: Length,
    /// Edit policy.
    pub edit_control: EditControl,
    /// Position to preserve for locked mechanical parts.
    pub locked_position: Option<Point>,
}

impl Footprint {
    /// Returns body bounds for placed footprints.
    pub fn body_bounds(&self) -> Option<BoundingBox> {
        let center = self.position?;
        let half_width = Length::from_micrometres(self.body_size.0.as_micrometres() / 2);
        let half_height = Length::from_micrometres(self.body_size.1.as_micrometres() / 2);
        Some(BoundingBox::new(
            Point {
                x: center.x - half_width,
                y: center.y - half_height,
            },
            Point {
                x: center.x + half_width,
                y: center.y + half_height,
            },
        ))
    }

    /// Returns courtyard bounds.
    pub fn courtyard_bounds(&self) -> Option<BoundingBox> {
        self.body_bounds()
            .map(|bounds| bounds.expanded(self.courtyard_margin))
    }
}

/// One footprint pad.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pad {
    /// Stable pad ID.
    pub id: String,
    /// Parent footprint ID.
    pub footprint_id: String,
    /// Pad number.
    pub number: String,
    /// Relative footprint coordinate.
    pub offset: Point,
    /// Cached absolute center, rebuilt after placement edits.
    pub center: Option<Point>,
    /// Copper size.
    pub size: (Length, Length),
    /// Drill diameter.
    pub drill: Length,
    /// Whether a drilled pad is plated.
    pub plated: bool,
    /// Assigned net ID.
    pub net_id: Option<String>,
    /// Solder mask expansion.
    pub mask_expansion: Length,
}

/// Net class and manufacturing rules.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetClass {
    /// Stable class ID.
    pub id: String,
    /// Track width.
    pub track_width: Length,
    /// Copper clearance.
    pub clearance: Length,
    /// Via diameter.
    pub via_diameter: Length,
    /// Via drill.
    pub via_drill: Length,
    /// Optional differential-pair gap model; Phase 0 reports it as uncovered.
    pub differential_gap: Option<Length>,
    /// Optional target length model; Phase 0 reports it as uncovered.
    pub target_length: Option<Length>,
}

/// A routed copper track.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Stable track ID.
    pub id: String,
    /// Net ID.
    pub net_id: String,
    /// Copper layer ID.
    pub layer_id: String,
    /// Track width.
    pub width: Length,
    /// Centerline geometry.
    pub path: Polyline,
    /// Provenance such as human, baseline-router or freerouting.
    pub provenance: String,
    /// Edit policy.
    pub edit_control: EditControl,
}

/// A through-board via candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Via {
    /// Stable via ID.
    pub id: String,
    /// Net ID.
    pub net_id: String,
    /// Via center.
    pub position: Point,
    /// Overall copper diameter.
    pub diameter: Length,
    /// Finished drill diameter.
    pub drill: Length,
    /// Edit policy.
    pub edit_control: EditControl,
}

/// A copper zone candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    /// Stable zone ID.
    pub id: String,
    /// Assigned net ID.
    pub net_id: String,
    /// Copper layer ID.
    pub layer_id: String,
    /// Unfilled boundary; fill is regenerated by the kernel/backend.
    pub boundary: Polygon,
    /// Minimum clearance.
    pub clearance: Length,
    /// Whether a fill candidate has been generated.
    pub filled: bool,
}

/// Routing/placement keepout.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keepout {
    /// Stable keepout ID.
    pub id: String,
    /// Boundary.
    pub boundary: Polygon,
    /// Whether copper is forbidden.
    pub copper: bool,
    /// Whether footprint placement is forbidden.
    pub placement: bool,
}

/// Silkscreen or fabrication text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextItem {
    /// Stable text ID.
    pub id: String,
    /// Layer ID/name.
    pub layer_id: String,
    /// Anchor point.
    pub position: Point,
    /// Rendered content.
    pub text: String,
    /// Approximate text bounds for deterministic checks.
    pub bounds: BoundingBox,
}

/// Versioned native PCB/DFM rules derived from a manufacturer profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcbRules {
    /// Rule-set version.
    pub version: String,
    /// Profile ID.
    pub profile_id: String,
    /// Minimum track width.
    pub minimum_track_width: Length,
    /// Minimum copper clearance.
    pub minimum_clearance: Length,
    /// Minimum drill diameter.
    pub minimum_drill: Length,
    /// Minimum annular ring.
    pub minimum_annular_ring: Length,
    /// Copper-to-edge clearance.
    pub minimum_edge_clearance: Length,
    /// Minimum solder-mask bridge.
    pub minimum_mask_bridge: Length,
    /// Minimum non-plated hole size.
    pub minimum_npth: Length,
    /// Maximum parser/artifact input size.
    pub maximum_artifact_bytes: u64,
    /// Evidence/source references.
    pub evidence_refs: Vec<String>,
}

impl Default for PcbRules {
    fn default() -> Self {
        Self {
            version: "jlcpcb-standard-v0-rules-1".to_owned(),
            profile_id: "jlcpcb-standard-v0".to_owned(),
            minimum_track_width: Length::from_micrometres(200),
            minimum_clearance: Length::from_micrometres(200),
            minimum_drill: Length::from_micrometres(300),
            minimum_annular_ring: Length::from_micrometres(150),
            minimum_edge_clearance: Length::from_micrometres(300),
            minimum_mask_bridge: Length::from_micrometres(100),
            minimum_npth: Length::from_micrometres(500),
            maximum_artifact_bytes: 16 * 1024 * 1024,
            evidence_refs: vec!["profile:jlcpcb-standard-v0".to_owned()],
        }
    }
}

/// Complete isolated PCB candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    /// Candidate schema version.
    pub schema_version: String,
    /// Source logical revision.
    pub source_revision: String,
    /// Candidate revision ID; not a project current pointer.
    pub candidate_revision: String,
    /// Board mechanics.
    pub definition: BoardDefinition,
    /// Active rules.
    pub rules: PcbRules,
    /// Footprints.
    pub footprints: Vec<Footprint>,
    /// Pads.
    pub pads: Vec<Pad>,
    /// Stable net names by ID.
    pub nets: BTreeMap<String, String>,
    /// Priority by net ID.
    pub net_priorities: BTreeMap<String, u16>,
    /// Net classes.
    pub net_classes: Vec<NetClass>,
    /// Routes.
    pub tracks: Vec<Track>,
    /// Vias.
    pub vias: Vec<Via>,
    /// Copper zones.
    pub zones: Vec<Zone>,
    /// Keepouts.
    pub keepouts: Vec<Keepout>,
    /// Silkscreen and fab text.
    pub texts: Vec<TextItem>,
    /// Logical pin ID to physical pad ID.
    pub pin_pad_map: BTreeMap<String, String>,
    /// Upstream evidence references.
    pub evidence_refs: Vec<String>,
}

impl Board {
    /// Applies a versioned manufacturer rule set to an isolated board candidate.
    #[must_use]
    pub fn apply_profile_candidate(&self, rules: PcbRules) -> Self {
        let mut board = self.clone();
        for class in &mut board.net_classes {
            class.track_width = class.track_width.max(rules.minimum_track_width);
            class.clearance = class.clearance.max(rules.minimum_clearance);
            class.via_drill = class.via_drill.max(rules.minimum_drill);
            let minimum_diameter = Length::from_micrometres(
                class.via_drill.as_micrometres() + 2 * rules.minimum_annular_ring.as_micrometres(),
            );
            class.via_diameter = class.via_diameter.max(minimum_diameter);
        }
        board.rules = rules;
        let hash = board.deterministic_hash();
        board.candidate_revision = format!("{}:profile:{}", self.candidate_revision, &hash[..12]);
        board
    }

    /// Marks valid zone boundaries as a deterministic native fill candidate.
    ///
    /// This Phase 0 operation records fill readiness and invalidates reference
    /// output; polygon boolean copper fill remains an explicit coverage gap.
    ///
    /// # Errors
    ///
    /// Returns [`PcbError::InvalidEdit`] if a zone boundary is invalid, outside
    /// the board, or uses clearance below the active profile minimum.
    pub fn zone_fill_candidate(&self) -> Result<Self, PcbError> {
        let mut board = self.clone();
        for zone in &mut board.zones {
            let validity = zone.boundary.validity();
            let bounds = zone.boundary.bounds();
            if !validity.valid
                || !board.definition.outline.contains(bounds.min)
                || !board.definition.outline.contains(bounds.max)
                || zone.clearance < board.rules.minimum_clearance
            {
                return Err(PcbError::InvalidEdit(format!(
                    "zone {} is not fillable under the active profile",
                    zone.id
                )));
            }
            zone.filled = true;
        }
        let hash = board.deterministic_hash();
        board.candidate_revision = format!("{}:zone-fill:{}", self.candidate_revision, &hash[..12]);
        Ok(board)
    }

    /// Constructs an uncommitted board candidate from a frozen logical netlist.
    ///
    /// # Errors
    ///
    /// Returns [`PcbError`] for an unsupported fixture version, invalid outline,
    /// missing exact footprint/pad mapping, or unknown logical references.
    #[allow(clippy::too_many_lines)]
    pub fn from_logical(
        logical: &LogicalNetlist,
        definition: BoardDefinition,
        rules: PcbRules,
    ) -> Result<Self, PcbError> {
        if logical.schema_version != LOGICAL_FIXTURE_VERSION {
            return Err(PcbError::UnsupportedLogicalVersion(
                logical.schema_version.clone(),
            ));
        }
        let validity = definition.outline.validity();
        if !validity.valid {
            return Err(PcbError::InvalidOutline(validity.issues.join(",")));
        }

        let net_ids: BTreeSet<_> = logical.nets.iter().map(|net| net.id.as_str()).collect();
        let mut footprints = Vec::with_capacity(logical.components.len());
        let mut pads = Vec::new();
        for component in &logical.components {
            let exact = component
                .footprint
                .as_ref()
                .ok_or_else(|| PcbError::MissingFootprint(component.id.clone()))?;
            footprints.push(Footprint {
                id: format!("pcb-footprint:{}", component.id),
                logical_component_id: component.id.clone(),
                designator: component.designator.clone(),
                value: component.value.clone(),
                manufacturer: component.manufacturer.clone(),
                mpn: component.mpn.clone(),
                footprint_name: exact.name.clone(),
                placement_role: component.placement_role.clone(),
                position: component.fixed_position,
                rotation: Angle::ZERO,
                side: Side::Front,
                body_size: (exact.body_width, exact.body_height),
                courtyard_margin: exact.courtyard_margin,
                height: exact.component_height,
                edit_control: if component.locked {
                    EditControl::Locked
                } else {
                    EditControl::AiEditable
                },
                locked_position: component.fixed_position.filter(|_| component.locked),
            });
            for pad in &exact.pads {
                pads.push(Pad {
                    id: format!("pcb-pad:{}:{}", component.id, pad.number),
                    footprint_id: format!("pcb-footprint:{}", component.id),
                    number: pad.number.clone(),
                    offset: pad.offset,
                    center: None,
                    size: (pad.width, pad.height),
                    drill: pad.drill,
                    plated: pad.plated,
                    net_id: None,
                    mask_expansion: Length::from_micrometres(50),
                });
            }
        }

        let mut pin_pad_map = BTreeMap::new();
        for pin in &logical.pins {
            if !net_ids.contains(pin.net_id.as_str()) {
                return Err(PcbError::UnknownNet {
                    pin_id: pin.id.clone(),
                    net_id: pin.net_id.clone(),
                });
            }
            let component = logical
                .components
                .iter()
                .find(|component| component.id == pin.component_id)
                .ok_or_else(|| PcbError::UnknownEntity(pin.component_id.clone()))?;
            let pad_number = component.pad_map.get(&pin.pin_number).ok_or_else(|| {
                PcbError::MissingPadMapping {
                    component_id: pin.component_id.clone(),
                    pin_number: pin.pin_number.clone(),
                }
            })?;
            let pad = pads
                .iter_mut()
                .find(|pad| {
                    pad.footprint_id == format!("pcb-footprint:{}", pin.component_id)
                        && pad.number == *pad_number
                })
                .ok_or_else(|| PcbError::UnknownPad {
                    component_id: pin.component_id.clone(),
                    pin_number: pin.pin_number.clone(),
                    pad_number: pad_number.clone(),
                })?;
            pad.net_id = Some(pin.net_id.clone());
            pin_pad_map.insert(pin.id.clone(), pad.id.clone());
        }

        let class_ids: BTreeSet<_> = logical
            .nets
            .iter()
            .map(|net| net.class_id.clone())
            .collect();
        let net_classes = class_ids
            .into_iter()
            .map(|id| NetClass {
                id,
                track_width: rules.minimum_track_width,
                clearance: rules.minimum_clearance,
                via_diameter: Length::from_micrometres(
                    rules.minimum_drill.as_micrometres()
                        + 2 * rules.minimum_annular_ring.as_micrometres(),
                ),
                via_drill: rules.minimum_drill,
                differential_gap: None,
                target_length: None,
            })
            .collect();
        let mut board = Self {
            schema_version: PCB_CANDIDATE_SCHEMA_VERSION.to_owned(),
            source_revision: logical.source_revision.clone(),
            candidate_revision: format!("pcb-candidate:{}", logical.source_revision),
            definition,
            rules,
            footprints,
            pads,
            nets: logical
                .nets
                .iter()
                .map(|net| (net.id.clone(), net.name.clone()))
                .collect(),
            net_priorities: logical
                .nets
                .iter()
                .map(|net| (net.id.clone(), net.priority))
                .collect(),
            net_classes,
            tracks: Vec::new(),
            vias: Vec::new(),
            zones: Vec::new(),
            keepouts: Vec::new(),
            texts: Vec::new(),
            pin_pad_map,
            evidence_refs: logical.evidence_refs.clone(),
        };
        board.rebuild_pad_centers();
        Ok(board)
    }

    /// Recomputes absolute pad centers after placement edits.
    pub fn rebuild_pad_centers(&mut self) {
        for pad in &mut self.pads {
            let Some(footprint) = self
                .footprints
                .iter()
                .find(|footprint| footprint.id == pad.footprint_id)
            else {
                pad.center = None;
                continue;
            };
            let Some(position) = footprint.position else {
                pad.center = None;
                continue;
            };
            let transform = Transform {
                translation: Vector {
                    dx: position.x,
                    dy: position.y,
                },
                rotation: footprint.rotation,
                mirror_x: footprint.side == Side::Back,
            };
            pad.center = Some(transform.apply(pad.offset));
        }
    }

    /// Returns a deterministic hash of the candidate input.
    ///
    /// # Panics
    ///
    /// Panics only if serde cannot serialize this statically serializable domain
    /// type, which indicates a programming defect rather than malformed input.
    pub fn deterministic_hash(&self) -> String {
        let encoded = serde_json::to_vec(self).expect("serializing Board cannot fail");
        hex::encode(Sha256::digest(encoded))
    }

    /// Returns the class for a net, falling back to the first class.
    pub fn class_for_net(&self, net_id: &str) -> Option<&NetClass> {
        let requested = self
            .nets
            .contains_key(net_id)
            .then_some("default")
            .and_then(|id| self.net_classes.iter().find(|class| class.id == id));
        requested.or_else(|| self.net_classes.first())
    }

    /// Returns all placed pad centers for a net in stable pad-ID order.
    pub fn pad_centers_for_net(&self, net_id: &str) -> Vec<(String, Point)> {
        let mut centers: Vec<_> = self
            .pads
            .iter()
            .filter(|pad| pad.net_id.as_deref() == Some(net_id))
            .filter_map(|pad| pad.center.map(|center| (pad.id.clone(), center)))
            .collect();
        centers.sort_by(|left, right| left.0.cmp(&right.0));
        centers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ComponentInstanceRef, FootprintDefinitionRef, LogicalNet, LogicalPinRef, PadDefinition,
    };

    fn definition() -> BoardDefinition {
        BoardDefinition {
            outline: Polygon::rectangle(
                Point::from_millimetres(0, 0),
                Point::from_millimetres(60, 40),
            ),
            cutouts: vec![],
            mounting_features: vec![],
            stackup: Stackup {
                board_thickness: Length::from_micrometres(1_600),
                layers: vec![],
                surface_finish: "HASL lead-free".to_owned(),
            },
            regions: vec![],
            origin: Point::from_millimetres(0, 0),
        }
    }

    fn logical(footprint: Option<FootprintDefinitionRef>) -> LogicalNetlist {
        LogicalNetlist {
            schema_version: LOGICAL_FIXTURE_VERSION.to_owned(),
            source_revision: "logical-rev-1".to_owned(),
            components: vec![ComponentInstanceRef {
                id: "component:R1".to_owned(),
                designator: "R1".to_owned(),
                value: "1k".to_owned(),
                manufacturer: "Yageo".to_owned(),
                mpn: "RC0603FR-071KL".to_owned(),
                footprint,
                pad_map: [("1".to_owned(), "1".to_owned())].into(),
                placement_role: "ordinary".to_owned(),
                locked: false,
                fixed_position: None,
            }],
            pins: vec![LogicalPinRef {
                id: "pin:R1:1".to_owned(),
                component_id: "component:R1".to_owned(),
                pin_number: "1".to_owned(),
                net_id: "net:GND".to_owned(),
            }],
            nets: vec![LogicalNet {
                id: "net:GND".to_owned(),
                name: "GND".to_owned(),
                class_id: "default".to_owned(),
                priority: 1,
            }],
            constraints: vec![],
            evidence_refs: vec![],
        }
    }

    fn footprint() -> FootprintDefinitionRef {
        FootprintDefinitionRef {
            library_id: "fixture".to_owned(),
            name: "R_0603_1608Metric".to_owned(),
            body_width: Length::from_micrometres(1_600),
            body_height: Length::from_micrometres(800),
            courtyard_margin: Length::from_micrometres(250),
            component_height: Length::from_micrometres(500),
            pads: vec![PadDefinition {
                number: "1".to_owned(),
                offset: Point::from_micrometres(-800, 0),
                width: Length::from_micrometres(800),
                height: Length::from_micrometres(900),
                drill: Length::ZERO,
                plated: false,
            }],
        }
    }

    #[test]
    fn missing_exact_footprint_blocks_board_creation() {
        assert_eq!(
            Board::from_logical(&logical(None), definition(), PcbRules::default()),
            Err(PcbError::MissingFootprint("component:R1".to_owned()))
        );
    }

    #[test]
    fn exact_mapping_creates_stable_pad_identity() {
        let board = Board::from_logical(
            &logical(Some(footprint())),
            definition(),
            PcbRules::default(),
        )
        .expect("valid board");
        assert_eq!(board.pin_pad_map["pin:R1:1"], "pcb-pad:component:R1:1");
    }
}
